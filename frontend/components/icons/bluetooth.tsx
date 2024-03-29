// https://thenounproject.com/hrsaxa/collection/aim-microphone-store-sex-bluetooth-wifi-chain-hear/?i=2091716
interface BluetoothProps {
	isConnected?: boolean;
	className?: string;
	['data-connected']?: string;
}

export default function Bluetooth(_props: BluetoothProps) {
	const { isConnected = false, ...props } = _props;
	if (isConnected) {
		props['data-connected'] = 'true';
	}
	return (
		<svg
			xmlns="http://www.w3.org/2000/svg"
			viewBox="0 0 100 100"
			{...props}
		>
			<path d="M16.72241,73.2937a4.00178,4.00178,0,0,0,5.57129.98346L46,57.68225V90.99994a4,4,0,0,0,6.21887,3.32819L82.21924,74.32794a4.00006,4.00006,0,0,0,.07519-6.60553L56.97583,49.99957l25.3186-17.72284a4.00006,4.00006,0,0,0-.07519-6.60553L52.21887,5.671A4.00018,4.00018,0,0,0,46,8.99915V42.31683L22.2937,25.722a4.00042,4.00042,0,0,0-4.58789,6.55475l25.3186,17.72284L17.70581,67.72241A4.00126,4.00126,0,0,0,16.72241,73.2937ZM54.00024,16.47284,72.91052,29.08038,54.00024,42.31683Zm0,41.20941L72.91052,70.9187,54.00024,83.52625Z" />
		</svg>
	);
}
