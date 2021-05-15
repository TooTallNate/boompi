import React from 'react';
import * as I from '@radix-ui/react-icons';

// HOC to remove the "width" and "height" attributes
function RemoveWidthHeight(El: React.SVGFactory) {
	return (props: React.ComponentProps<'svg'>) => {
		return <El width={undefined} height={undefined} {...props} />;
	};
}

export const Play = RemoveWidthHeight(I.PlayIcon);
export const Pause = RemoveWidthHeight(I.PauseIcon);
export const TrackNext = RemoveWidthHeight(I.TrackNextIcon);
export const TrackPrevious = RemoveWidthHeight(I.TrackPreviousIcon);
export const SpeakerLoudIcon = RemoveWidthHeight(I.SpeakerLoudIcon);
export const SpeakerModerateIcon = RemoveWidthHeight(I.SpeakerModerateIcon);
export const SpeakerOffIcon = RemoveWidthHeight(I.SpeakerOffIcon);
export const SpeakerQuietIcon = RemoveWidthHeight(I.SpeakerQuietIcon);
